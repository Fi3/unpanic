//! This is the core of the hir traverser. It recursivley check an Node (either a Block an Expr or
//! a Stmt) for call to function that contains panics.

use crate::utils::log_allow_panic;
use rustc_hir::HirId;
use rustc_hir::{Block, Expr, ExprKind, Guard, StmtKind,PathSegment,def_id::DefId,QPath,def::{Res,DefKind},Node};
use rustc_span::Span;
use rustc_middle::hir::map::Map;
use rustc_middle::ty::TyCtxt;
use std::collections::HashMap;
use rustc_type_ir::sty::TyKind;

use super::ForeignCallsToCheck;

#[derive(Debug,Eq,PartialEq,Hash)]
pub enum HirId_ {
    Local(HirId),
    Extern(HirId),
}
impl PartialEq<HirId_> for HirId {
    fn eq(&self, other: &HirId_) -> bool {
        self == &other.inner()
    }
}
impl PartialEq<HirId> for HirId_ {
    fn eq(&self, other: &HirId) -> bool {
        &self.inner() == other
    }
}

impl HirId_ {
    pub fn inner(&self) -> HirId {
        match self {
            HirId_::Local(id) => *id,
            HirId_::Extern(id) => *id,
        }
    }
    pub fn is_extern(&self) -> bool {
        match self {
            HirId_::Local(_) => false,
            HirId_::Extern(_) => true,
        }
    }
}


/// Traverse the HIR and collect function call with therir stack from the starting point.
/// eg if we have
/// f1
/// |-> f2
/// |-> f3
/// |    |-> f4
/// |    |-> f5
/// |    |-> f2 // this will be ignored because we already visited f2
/// |->f6
///
/// It collect:
/// f2 (f1,f2)
/// f3 (f1,f3)
/// f4 (f1,f3,f4)
/// f5 (f1,f3,f5)
/// f6 (f1,f6)
///
/// All the leaf are calls to extern functions and are marked as Extern
///
/// If save_stack is false it just collect the first level fucntion call without the stack
/// Using the example above it will collect:
/// f2 ()
/// f3 ()
/// f6 ()
///
pub struct FunctionCallPartialTree<'tcx> {
    pub tcx: TyCtxt<'tcx>,
    pub visited_functions: HashMap<HirId_, (DefId,String,Vec<String>)>,
    pub visited_assoc_functions: HashMap<HirId_,(DefId,Option<DefId>,Vec<String>)>,
    /// For each allow_panic that we encounter we save the call_stack
    pub allow_panics: Vec<Vec<String>>,
    pub save_stack: bool,
    pub first_level_calls: Vec<Expr<'tcx>>,
}

impl<'tcx> FunctionCallPartialTree<'tcx> {

    pub fn new(tcx: TyCtxt<'tcx>,save_stack: bool) -> Self {
        Self {
           tcx,
           visited_functions: HashMap::new(),
           visited_assoc_functions: HashMap::new(),
           /// For each allow_panic that we encounter we save the call_stack
           allow_panics: Vec::new(),
           save_stack,
           first_level_calls: Vec::new(),
        }
    }

    pub fn traverse_block(
        &mut self,
        block: &Block<'tcx>,
        call_stack: &mut Vec<String>,
    ) {
        let hir_krate = self.tcx.hir();
        for stmt in block.stmts {
            self.traverse_stmt(&stmt.kind,call_stack);
        }
        if let Some(expr) = block.expr {
            self.traverse_expr(expr,call_stack);
        }
    }
    pub fn traverse_stmt(
        &mut self,
        stmt: &StmtKind<'tcx>,
        call_stack: &mut Vec<String>,
    ) {
        let hir_krate = self.tcx.hir();
        match stmt {
            StmtKind::Local(local) => {
                if let Some(expr) = local.init {
                    self.traverse_expr(expr,call_stack);
                }
                if let Some(block) = local.els {
                    for stmt in block.stmts {
                        self.traverse_stmt(&stmt.kind,call_stack);
                    }
                    if let Some(expr) = block.expr {
                        self.traverse_expr(expr,call_stack);
                    }
                }
            }
            StmtKind::Item(_) => (),
            StmtKind::Expr(expr) => self.traverse_expr(expr,call_stack),
            StmtKind::Semi(expr) => self.traverse_expr(expr,call_stack),
        }
    }
    pub fn traverse_expr(
        &mut self,
        expr_: &Expr<'tcx>,
        call_stack: &mut Vec<String>,
    ) {
        let hir_krate = self.tcx.hir();
        match expr_.kind {
            ExprKind::ConstBlock(const_block) => {
                let expr = hir_krate.body(const_block.body).value;
                self.traverse_expr(expr,call_stack);
            }
            ExprKind::Array(array) => {
                for expr in array {
                    self.traverse_expr(expr,call_stack);
                }
            }
            ExprKind::Call(call, args) => {
                if ! self.save_stack {
                    self.first_level_calls.push(expr_.clone());
                    for arg in args {
                        self.traverse_expr(arg,call_stack);
                    }
                    return;
                }
                // I do not want to visit 2 times the same function even if they are called in
                // different places. I need just the first occurence of a fucntion. If the first
                // occurence contains a panic this is an error and there is no need to check the
                // second occurence. 
                if ! (self.visited_functions.contains_key(&HirId_::Local(call.hir_id.owner.into())) 
                      || self.visited_functions.contains_key(&HirId_::Extern(call.hir_id.owner.into())))
                {
                    self.traverse_expr(call,call_stack);
                    // TODO args should be checked outside if clause
                    for expr in args {
                        self.traverse_expr(expr,call_stack);
                    }
                }
            }
            ExprKind::MethodCall(method, receiver, args, span) => {
                if ! self.save_stack {
                    self.first_level_calls.push(expr_.clone());
                    for arg in args {
                        self.traverse_expr(arg,call_stack);
                    }
                    return;
                }
                if ! (self.visited_assoc_functions.contains_key(&HirId_::Local(method.hir_id.owner.into())) 
                      || self.visited_assoc_functions.contains_key(&HirId_::Extern(method.hir_id.owner.into()))) 
                    && self.save_stack
                {
                    let result = self.tcx.typeck(receiver.hir_id.owner.def_id);
                    let ty = result.expr_ty(receiver);
                    let def_id = result
                        .type_dependent_def_id(expr_.hir_id)
                        .expect("ERROR: Can not get def id");
                    if ! def_id.is_local() {
                        match ty.kind() {
                            rustc_middle::ty::Adt(adt_def, _) => {
                                Self::add_to_stack(span,call_stack);
                                self.visited_assoc_functions.insert(
                                    HirId_::Extern(method.hir_id.owner.into()),
                                    (
                                        def_id,
                                        Some(adt_def.did()),
                                        call_stack.to_vec(),
                                    )
                                );
                            },
                            _ => self.traverse_expr(receiver,call_stack),
                        };
                    } else {
                        self.traverse_expr(receiver,call_stack);
                    }
                    for expr in args {
                        self.traverse_expr(expr,call_stack);
                    }
                }
            }
            ExprKind::Tup(tup) => {
                for expr in tup {
                    self.traverse_expr(expr,call_stack);
                }
            }
            ExprKind::Binary(_, arg1, arg2) => {
                self.traverse_expr(arg1,call_stack);
                self.traverse_expr(arg2,call_stack);
            }
            ExprKind::Unary(_, arg) => {
                self.traverse_expr(arg,call_stack);
            }
            ExprKind::Lit(_) => (),
            ExprKind::Cast(expr, _) => self.traverse_expr(expr,call_stack),
            ExprKind::Type(expr, _) => {
                self.traverse_expr(expr,call_stack);
            }
            ExprKind::DropTemps(expr) => {
                self.traverse_expr(expr,call_stack);
            }
            ExprKind::Let(let_) => {
                self.traverse_expr(let_.init,call_stack);
            }
            ExprKind::If(cond, if_block, Some(else_block)) => {
                self.traverse_expr(cond,call_stack);
                self.traverse_expr(if_block,call_stack);
                self.traverse_expr(else_block,call_stack);
            }
            ExprKind::If(cond, if_block, None) => {
                self.traverse_expr(cond,call_stack);
                self.traverse_expr(if_block,call_stack);
            }
            ExprKind::Loop(block, _, _, _) => {
                self.traverse_block(block,call_stack);
            }
            ExprKind::Match(expr, arms, _) => {
                self.traverse_expr(expr,call_stack);
                for arm in arms {
                    match arm.guard {
                        Some(Guard::If(expr)) => self.traverse_expr(expr,call_stack),
                        Some(Guard::IfLet(let_)) => self.traverse_expr(let_.init,call_stack),
                        None => (),
                    };
                    self.traverse_expr(arm.body,call_stack);
                }
            }
            ExprKind::Closure(closure) => {
                let expr = hir_krate.body(closure.body).value;
                self.traverse_expr(expr,call_stack);
            }
            ExprKind::Block(block, Some(label)) => {
                if !label.ident.as_str().contains("allow_panic") {
                    self.traverse_block(block,call_stack);
                } else {
                    self.allow_panics.push(call_stack.clone());
                }
            }
            ExprKind::Block(block, None) => {
                self.traverse_block(block,call_stack);
            }
            ExprKind::Assign(arg1, arg2, _) => {
                self.traverse_expr(arg1,call_stack);
                self.traverse_expr(arg2,call_stack);
            }
            ExprKind::AssignOp(_, arg1, arg2) => {
                self.traverse_expr(arg1,call_stack);
                self.traverse_expr(arg2,call_stack);
            }
            ExprKind::Field(expr, _) => {
                self.traverse_expr(expr,call_stack);
            }
            ExprKind::Index(arg1, arg2, _) => {
                self.traverse_expr(arg1, call_stack);
                self.traverse_expr(arg2, call_stack);
            }
            ExprKind::Path(path) => match path{
                QPath::Resolved(_,path) => {
                    if let Some(last) = path.segments.last() {
                        let fn_ident = last.ident.as_str().to_string();
                        Self::add_to_stack(path.span,call_stack);
                        match last.res {
                            Res::Def(DefKind::Fn, def_id) => {
                                if let Some(local_id) = def_id.as_local() {
                                    let item = hir_krate.expect_item(local_id);
                                    if let rustc_hir::ItemKind::Fn(_, _, body_id) = item.kind {
                                        let hir_ = HirId_::Local(last.hir_id.owner.into());
                                        self.visited_functions.insert(hir_, (def_id,fn_ident,call_stack.clone()));
                                        let expr = hir_krate.body(body_id).value;
                                        self.traverse_expr(expr,call_stack);
                                    } else {
                                        panic!()
                                    }
                                } else {
                                    // Extern function are the leafs of the call tree
                                    let hir_ = HirId_::Extern(last.hir_id.owner.into());
                                    self.visited_functions.insert(hir_, (def_id,fn_ident,call_stack.clone()));
                                }
                            },
                            // Constructors are conidered safe
                            Res::Def(DefKind::Ctor(_,_), _) => (),
                            // This will be handled by the second pass TODO check if comment is correct
                            Res::Local(_) => {
                                //dbg!(expr_);
                            },
                            _ => (),
                        }
                    } else {
                        panic!()
                    }
                },
                QPath::TypeRelative(_, segment) => {
                    let result = self.tcx.typeck(segment.hir_id.owner.def_id);
                    let items = result.node_types().items_in_stable_order();
                    for item in items {
                        if let TyKind::FnDef(def_id, generic_args) = item.1.kind() {
                            // TODO this seems to be correct the receiver is always the first
                            // genarg but check it.
                            let receiver = generic_args.get(0).map(
                                // TODO here we should always have a ty but we dont
                                |x| x.as_type().map(
                                    |ty| match ty.kind() {
                                        rustc_middle::ty::Adt(adt_def, _) => Some(adt_def.did()),
                                        // TODO we should never reach this point but we do
                                        _ => None,
                                    })
                            ).flatten().flatten();
                            // If is a trait fn and is implemented solve it and visit the
                            // implementation
                            if let Some(impl_item) = super::get_impl_item(
                                &mut self.tcx,
                                *def_id,
                                // TODO this seems to be correct the receiver is always the first
                                // genarg but check it. It should be expect_type
                                generic_args.get(0).map(|x| x.as_type()).flatten(),
                            ) {
                                if let Some(local_id) = impl_item.def_id.as_local() {
                                    match hir_krate.get_by_def_id(local_id) {
                                        // TraitItem are handled elsewhere TODO
                                        Node::TraitItem(_) => (),
                                        Node::ImplItem(item) => {
                                            if let rustc_hir::ImplItemKind::Fn(_, body_id) = item.kind {
                                                let expr = hir_krate.body(body_id).value;
                                                Self::add_to_stack(expr.span,call_stack);
                                                self.visited_assoc_functions.insert(
                                                    HirId_::Local(segment.hir_id.owner.into()),
                                                    (
                                                        *def_id,
                                                        receiver,
                                                        call_stack.to_vec(),
                                                    )
                                                );
                                                self.traverse_expr(expr, call_stack);
                                            }
                                        }
                                        item => panic!("Unexpected Node {:?}", item),
                                    }
                                } else {
                                    unreachable!()
                                }
                            // If is local check if the function contains calls to panic
                            } else if let Some(local_id) = def_id.as_local() {
                                if let Some(Node::Item(item)) = hir_krate.find_by_def_id(local_id) {
                                    if let rustc_hir::ItemKind::Fn(_, _, body_id) = item.kind {
                                        let expr = hir_krate.body(body_id).value;
                                        Self::add_to_stack(expr.span,call_stack);
                                        self.visited_assoc_functions.insert(
                                            HirId_::Local(segment.hir_id.owner.into()),
                                            (
                                                *def_id,
                                                receiver,
                                                call_stack.to_vec(),
                                            )
                                        );
                                        self.traverse_expr(expr, call_stack);
                                    }
                                };
                                if let Some(Node::ImplItem(item)) = hir_krate.find_by_def_id(local_id) {
                                    if let rustc_hir::ImplItemKind::Fn(_, body_id) = item.kind {
                                        let expr = hir_krate.body(body_id).value;
                                        Self::add_to_stack(expr.span,call_stack);
                                        self.visited_assoc_functions.insert(
                                            HirId_::Local(segment.hir_id.owner.into()),
                                            (
                                                *def_id,
                                                receiver,
                                                call_stack.to_vec(),
                                            )
                                        );
                                        self.traverse_expr(expr, call_stack);
                                    }
                                };
                            // Otherwise save it for later check
                            } else {
                                Self::add_to_stack(expr_.span,call_stack);
                                self.visited_assoc_functions.insert(
                                    HirId_::Extern(segment.hir_id.owner.into()),
                                    (
                                        *def_id,
                                        receiver,
                                        call_stack.to_vec(),
                                    )
                                );
                                // TODO todo
                            }
                        }
                    }
                },
                // TODO Maybe add a field in the config file to check also for not wanted LangItem
                QPath::LangItem(_, _, _) => (),
            },
            ExprKind::AddrOf(_, _, expr) => {
                self.traverse_expr(expr,call_stack);
            }
            ExprKind::Break(_, Some(expr)) => {
                self.traverse_expr(expr,call_stack);
            }
            ExprKind::Break(_, None) => (),
            ExprKind::Continue(_) => (),
            ExprKind::Ret(Some(expr)) => {
                self.traverse_expr(expr,call_stack);
            }
            ExprKind::Ret(None) => (),
            ExprKind::InlineAsm(_) => (),
            ExprKind::OffsetOf(_, _) => (),
            ExprKind::Struct(_, fields, Some(base)) => {
                self.traverse_expr(base,call_stack);
                for field in fields {
                    self.traverse_expr(field.expr,call_stack);
                }
            }
            ExprKind::Struct(_, fields, None) => {
                for field in fields {
                    self.traverse_expr(field.expr,call_stack);
                }
            }
            ExprKind::Repeat(elem, _) => {
                self.traverse_expr(elem,call_stack);
            }
            ExprKind::Yield(expr, _) => {
                self.traverse_expr(expr,call_stack);
            }
            ExprKind::Become(_) => todo!(),
            // TODO why we return here? Why an ExprKind:Err is found?
            ExprKind::Err(_) => return,
        }
    }

    fn add_to_stack(function: Span, call_stack: &mut Vec<String>) {
        let formatted = format!("{:?}",function);
        call_stack.push(formatted);
    }
}
