use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, LitStr};

pub fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            Fields::Unnamed(fields) => {
                return Err(syn::Error::new_spanned(
                    fields,
                    "Chainable requires a struct with named fields",
                ));
            }
            Fields::Unit => {
                return Err(syn::Error::new_spanned(
                    name,
                    "Chainable requires a struct with named fields",
                ));
            }
        },
        Data::Enum(data) => {
            return Err(syn::Error::new_spanned(
                &data.enum_token,
                "Chainable can only be derived for structs",
            ));
        }
        Data::Union(data) => {
            return Err(syn::Error::new_spanned(
                &data.union_token,
                "Chainable can only be derived for structs",
            ));
        }
    };

    let setters = fields
        .iter()
        .filter_map(|field| match is_skipped(field) {
            Ok(true) => None,
            Ok(false) => Some(Ok(field)),
            Err(error) => Some(Err(error)),
        })
        .map(|field| {
            let field = field?;
            let field_name = field.ident.as_ref().expect("named fields have identifiers");
            let field_type = &field.ty;
            let visibility = &field.vis;
            let documentation = LitStr::new(
                &format!("Sets [`Self::{field_name}`] and returns the updated value."),
                field_name.span(),
            );

            Ok(quote! {
                #[doc = #documentation]
                #[inline]
                #visibility fn #field_name(mut self, value: #field_type) -> Self {
                    self.#field_name = value;
                    self
                }
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #name #type_generics #where_clause {
            #(#setters)*
        }
    })
}

fn is_skipped(field: &syn::Field) -> syn::Result<bool> {
    let mut skipped = false;
    for attribute in field
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("chain"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                if skipped {
                    return Err(meta.error("duplicate `skip` option"));
                }
                skipped = true;
                Ok(())
            } else {
                Err(meta.error("unknown option; expected `skip`"))
            }
        })?;
    }
    Ok(skipped)
}
