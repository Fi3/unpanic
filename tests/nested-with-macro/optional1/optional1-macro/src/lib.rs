extern crate proc_macro;
use proc_macro::{Group, TokenStream, TokenTree};

#[proc_macro_derive(Panicable)]
pub fn panicable(item: TokenStream) -> TokenStream {
    let name = get_struct_name(item);
    format!(
        "mod impl_panicable {{
    use super::*;


    impl Panicable for {} {{
        fn panic_now() {{
            panic!()
        }}
    }}

    }}",
        name,
    )
    .parse()
    .unwrap()
}

fn get_struct_name(item: TokenStream) -> String {
    let item = remove_attributes(item);
    let mut stream = item.into_iter();

    // Check if the stream is a struct
    loop {
        match stream.next().expect("Stream not a struct") {
            TokenTree::Ident(i) => {
                if i.to_string() == "struct" {
                    break;
                }
            }
            _ => continue,
        }
    }

    // Get the struct name
    let struct_name = match stream.next().expect("Struct has no name") {
        TokenTree::Ident(i) => i.to_string(),
        // Never executed at runtime it ok to panic
        _ => panic!("Strcut has no name"),
    };
    struct_name
}

fn remove_attributes(item: TokenStream) -> TokenStream {
    let stream = item.into_iter();
    let mut is_attribute = false;
    let mut result = Vec::new();

    for next in stream {
        match next.clone() {
            TokenTree::Punct(p) => {
                if p.to_string() == "#" {
                    is_attribute = true;
                } else {
                    result.push(next.clone());
                }
            }
            TokenTree::Group(g) => {
                if is_attribute {
                    continue;
                } else {
                    let delimiter = g.delimiter();
                    let cleaned_group = remove_attributes(g.stream());
                    let cleaned_group = TokenTree::Group(Group::new(delimiter, cleaned_group));
                    result.push(cleaned_group);
                }
            }
            _ => {
                is_attribute = false;
                result.push(next.clone());
            }
        }
    }

    TokenStream::from_iter(result)
}
