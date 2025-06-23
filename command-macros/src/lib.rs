use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn command_handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    
    let expanded = quote! {
        #input_fn
        
        paste::paste! {
            #[ctor::ctor]
            fn [<__register_command_ #fn_name>]() {
                crate::controller::discord::interaction::register_command(
                    #fn_name_str,
                    |data, app_state| Box::pin(#fn_name(data, app_state))
                );
            }
        }
    };
    
    TokenStream::from(expanded)
}
