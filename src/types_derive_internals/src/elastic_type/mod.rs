use quote::Tokens;
use syn;
use serde_derive_internals::{self, attr as serde_attr};
use super::{get_elastic_meta_items, expect_list, expect_name_value, expect_ident, get_ident_from_lit, get_tokens_from_lit};

struct ElasticDocumentMapping {
    ident: syn::Ident,
    definition: Tokens,
    impl_block: Tokens,
}

/**
Derive `DocumentType` for the given input.

The input must satisfy the following rules:

- It must be a struct.
- The structs field types must implement `FieldType` (or be ignored).
- A mapping type supplied by `#[elastic(mapping="<ident>")]` must implement `DocumentMapping`,
but not `PropertiesMapping`.
*/
pub fn expand_derive(crate_root: Tokens, input: &syn::MacroInput) -> Result<Vec<Tokens>, DeriveElasticTypeError> {
    // Annotatable item for a struct with struct fields
    let fields = match input.body {
        syn::Body::Struct(ref data) => match *data {
            syn::VariantData::Struct(ref fields) => Some(fields),
            _ => None,
        },
        _ => None,
    };

    let fields = fields.ok_or(DeriveElasticTypeError::InvalidInput)?;

    // Get the serializable fields
    let fields: Vec<(syn::Ident, &syn::Field)> = fields
        .iter()
        .map(|f| get_ser_field(f))
        .filter(|f| f.is_some())
        .map(|f| f.unwrap())
        .collect();

    let mapping = get_mapping(&crate_root, input);

    let doc_ty_impl_block = get_doc_ty_impl_block(
        &crate_root,
        input,
        &fields,
        &mapping.ident,
        &mapping.ident);

    let props_impl_block = get_props_impl_block(
        &crate_root,
        &mapping.ident,
        &fields);

    let dummy_wrapper = syn::Ident::new(format!("_IMPL_EASTIC_TYPE_FOR_{}", input.ident));

    let mapping_definition = &mapping.definition;
    let mapping_impl_block = &mapping.impl_block;

    Ok(vec![
        quote!(
        #mapping_definition

        #[allow(non_upper_case_globals, dead_code, unused_variables)]
        const #dummy_wrapper: () = {            
            #mapping_impl_block

            #doc_ty_impl_block

            #props_impl_block
        };
    ),
    ])
}

fn get_mapping(crate_root: &Tokens, input: &syn::MacroInput) -> ElasticDocumentMapping {
    // Define a struct for the mapping with a few defaults
    fn define_mapping(name: &syn::Ident) -> Tokens {
        quote!(
            #[derive(Default, Clone, Copy, Debug)]
            pub struct #name;
        )
    }

    // Get the default mapping name
    fn get_default_mapping(item: &syn::MacroInput) -> syn::Ident {
        syn::Ident::from(format!("{}Mapping", item.ident))
    }

    // Get the mapping ident supplied by an #[elastic()] attribute or create a default one
    fn get_mapping_from_attr(item: &syn::MacroInput) -> Option<syn::Ident> {
        let val = get_elastic_meta_items(&item.attrs);
            
        let val = val
            .iter()
            .filter_map(|meta| expect_name_value("mapping", &meta))
            .next();

        val.and_then(|v| get_ident_from_lit(v).ok())
    }

    // Implement DocumentMapping for the mapping
    fn impl_document_mapping(crate_root: &Tokens, mapping: &syn::Ident, properties: &syn::Ident) -> Tokens {
        quote!(
            impl #crate_root::derive::DocumentMapping for #mapping {
                type Properties = #properties;
            }
        )
    }

    if let Some(ident) = get_mapping_from_attr(input) {
        ElasticDocumentMapping {
            ident,
            definition: Tokens::new(),
            impl_block: Tokens::new(),
        }
    } else {
        let ident = get_default_mapping(input);
        let definition = define_mapping(&ident);
        let impl_block = impl_document_mapping(&crate_root, &ident, &ident);

        ElasticDocumentMapping {
            ident,
            definition,
            impl_block,
        }
    }
}

// Implement DocumentType for the type being derived with the mapping
fn get_doc_ty_impl_block(
    crate_root: &Tokens,
    item: &syn::MacroInput,
    fields: &[(syn::Ident, &syn::Field)],
    mapping: &syn::Ident,
    properties: &syn::Ident)
-> Tokens {
    struct ElasticDocumentTypeMethods {
        index: Tokens,
        ty: Tokens,
        id: Tokens,
    }

    // Get the default method blocks for `DocumentType`
    fn get_doc_type_methods(item: &syn::MacroInput, fields: &[(syn::Ident, &syn::Field)]) -> ElasticDocumentTypeMethods {
        // Get the default name for the indexed elasticsearch type name
        fn get_elastic_type_name(item: &syn::MacroInput) -> syn::Lit {
            syn::Lit::Str(
                format!("{}", item.ident).to_lowercase(),
                syn::StrStyle::Cooked,
            )
        }

        // Get the mapping ident supplied by an #[elastic()] attribute or create a default one
        // Parses #[elastic(method = $lit)]
        // Parses #[elastic(method(expr = $expr))]
        fn get_method_from_struct(item: &syn::MacroInput, method: &str) -> Option<Tokens> {
            let val = get_elastic_meta_items(&item.attrs);
            
            // Attempt to get a literal 
            if let Some(lit) = val
                .iter()
                .filter_map(|meta| expect_name_value(method, meta))
                .next()
            {
                return Some(quote!(#lit))
            }

            if let Some(expr) = val
                .iter()
                .filter_map(|meta| expect_list(method, meta))
                .flat_map(|attrs| attrs)
                .filter_map(|meta| expect_name_value("expr", meta))
                .next()
                .and_then(|expr| get_tokens_from_lit(expr).ok())
            {
                return Some(quote!(#expr (self)))
            }

            None
        }

        fn get_method_from_fields(fields: &[(syn::Ident, &syn::Field)], method: &str) -> Option<Tokens> {
            for &(_, ref field) in fields {
                if get_elastic_meta_items(&field.attrs)
                    .iter()
                    .any(|meta| {
                        expect_ident(method, meta)
                    })
                {
                    let field = &field.ident;

                    return  Some(quote!(&self . #field));
                }
            }
            
            None
        }

        let index_expr = get_method_from_struct(item, "index")
            .unwrap_or_else(|| {
                let name = get_elastic_type_name(item);
                quote!(#name)
            });

        let ty = get_method_from_struct(item, "ty")
            .map(|tokens| quote!(
                fn ty(&self) -> ::std::borrow::Cow<str> {
                    (#tokens).into()
                }
            ))
            .unwrap_or_else(Tokens::new);

        let id = get_method_from_struct(item, "id")
            .or_else(|| get_method_from_fields(fields, "id"))
            .map(|id_expr| quote!(
                fn id(&self) -> Option<::std::borrow::Cow<str>> {
                    Some((#id_expr).into())
                }
            ))
            .unwrap_or_else(Tokens::new);

        ElasticDocumentTypeMethods {
            index: quote!(
                fn index(&self) -> ::std::borrow::Cow<str> {
                    (#index_expr).into()
                }
            ),
            ty,
            id,
        }
    }

    let doc_ty = &item.ident;
    let ElasticDocumentTypeMethods {
        ref index,
        ref ty,
        ref id,
    } = get_doc_type_methods(item, fields);

    quote!(
        impl #crate_root::derive::DocumentType for #doc_ty {
            type Mapping = #mapping;
            type Properties = #properties;

            #index

            #ty

            #id
        }
    )
}

// Implement PropertiesMapping for the mapping
fn get_props_impl_block(
    crate_root: &Tokens,
    props_ty: &syn::Ident,
    fields: &[(syn::Ident, &syn::Field)])
-> Tokens {
    // Get the serde serialisation statements for each of the fields on the type being derived
    fn get_field_ser_stmts(crate_root: &Tokens, fields: &[(syn::Ident, &syn::Field)]) -> Vec<Tokens> {
        let fields: Vec<Tokens> = fields
            .iter()
            .cloned()
            .map(|(name, field)| {
                let lit = syn::Lit::Str(name.as_ref().to_string(), syn::StrStyle::Cooked);
                let ty = &field.ty;

                let expr = quote!(#crate_root::derive::mapping::<#ty, _, _>());

                quote!(try!(#crate_root::derive::field_ser(state, #lit, #expr));)
            })
            .collect();

        fields
    }

    let stmts = get_field_ser_stmts(crate_root, fields);
    let stmts_len = stmts.len();

    quote!(
        impl #crate_root::derive::PropertiesMapping for #props_ty {
            fn props_len() -> usize { #stmts_len }

            fn serialize_props<S>(state: &mut S) -> ::std::result::Result<(), S::Error> 
                where S: #crate_root::derive::SerializeStruct {
                #(#stmts)*
                Ok(())
            }
        }
    )
}

fn get_ser_field(field: &syn::Field) -> Option<(syn::Ident, &syn::Field)> {
    let ctxt = serde_derive_internals::Ctxt::new();
    let serde_field = serde_attr::Field::from_ast(&ctxt, 0, field);

    // If the `serde` parse fails, return `None` and let `serde` panic later
    match ctxt.check() {
        Err(_) => return None,
        _ => (),
    };

    // Get all fields on struct where there isn't `skip_serializing`
    if serde_field.skip_serializing() {
        return None;
    }

    Some((
        syn::Ident::from(serde_field.name().serialize_name().as_ref()),
        field,
    ))
}

quick_error! {
    #[derive(Debug)]
    pub enum DeriveElasticTypeError {
        InvalidInput {
            display("deriving a document type is only valid for structs")
        }
    }
}
