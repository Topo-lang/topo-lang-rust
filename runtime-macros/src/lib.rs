//! Proc macros for the Topo runtime.
//!
//! Provides `#[topo_pipeline]` which transforms a function into a pipeline
//! placeholder that PipelineCodeGenPass will replace at the IR level.
//!
//! The macro upholds a frozen contract — accepted attr surface (currently
//! none — `#[topo_pipeline]` / `#[topo_pipeline()]` only), accepted body
//! shape (empty `{}` only; the IR pass replaces the whole body so any
//! user-written statement would be silently dropped), and a closed set
//! of compile-error categories. It is the single consumer-facing entry
//! to the topo IR pipeline runtime; any input it cannot honour MUST
//! produce a `compile_error!` rather than silently transform the wrong
//! thing.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, Block, ItemFn, ReturnType, Stmt};

/// Marks a function as a Topo pipeline placeholder.
///
/// The function body is replaced with a call to `topo::pipeline::placeholder()`,
/// which PipelineCodeGenPass detects and replaces with generated pipeline logic.
///
/// # Contract
///
/// - **Attribute args**: the attribute MUST be invoked with empty parens or
///   no parens at all. `#[topo_pipeline]` and `#[topo_pipeline()]` are
///   accepted; any other token in the attribute slot is a hard error so
///   future-extension space stays open without ambiguity.
/// - **Body shape**: the function body MUST be empty (`{}`). Writing
///   anything else is a hard error: the placeholder design replaces the
///   whole body at IR level, so a user-written body would be silently
///   dropped at runtime -- the proc macro surfaces that mismatch at
///   compile time instead.
///
/// See the module-level docs (`//!` block at the top of this file) for
/// the contract summary.
///
/// # Example
///
/// ```rust,ignore
/// #[topo::topo_pipeline]
/// pub fn process(input: i32) -> i32 {}
/// ```
///
/// Expands to:
///
/// ```rust,ignore
/// pub fn process(input: i32) -> i32 {
///     topo::pipeline::placeholder::<i32>()
/// }
/// ```
#[proc_macro_attribute]
pub fn topo_pipeline(attr: TokenStream, item: TokenStream) -> TokenStream {
    // ---- Attr surface check (closed today, extensible tomorrow) -----------
    //
    // The macro accepts zero attribute tokens. Any other input is rejected
    // with a `compile_error!` pointing at the unexpected tokens; this
    // keeps the attr namespace explicitly closed so a future addition
    // (e.g. `#[topo_pipeline(stage = "ingest")]`) can be introduced with
    // a real surface, never confused with a pre-existing silent drop.
    let attr2: TokenStream2 = attr.into();
    if !attr2.is_empty() {
        let span = attr2.span();
        let msg = format!(
            "#[topo_pipeline] takes no arguments today (got `{}`)",
            attr2
        );
        return quote_spanned! { span => compile_error!(#msg); }.into();
    }

    let input = parse_macro_input!(item as ItemFn);

    // ---- Body shape check (placeholder design: body MUST be empty) -------
    //
    // The IR pass replaces the *whole* function body, so any user-written
    // statement would be dropped at compile time without warning. That
    // failure mode is exactly the contract drift this proc macro now
    // surfaces: emit a `compile_error!` pointing at the body so the user
    // either deletes it or learns the placeholder contract.
    if let Some(err) = check_empty_body(&input.block) {
        return err.into();
    }

    let vis = &input.vis;
    let sig = &input.sig;
    let attrs = &input.attrs;

    // Extract return type for the placeholder generic parameter
    let body = match &sig.output {
        ReturnType::Default => {
            // void return -- just call placeholder::<()>()
            quote! { topo::pipeline::placeholder::<()>() }
        }
        ReturnType::Type(_, ty) => {
            quote! { topo::pipeline::placeholder::<#ty>() }
        }
    };

    let output = quote! {
        #(#attrs)*
        #vis #sig {
            #body
        }
    };

    output.into()
}

/// Returns `Some(error_token_stream)` if the body contains anything other
/// than an empty block; otherwise `None`. The check tolerates an
/// all-`Stmt::Item` body of zero items (i.e. a literal `{}`).
fn check_empty_body(block: &Block) -> Option<TokenStream2> {
    let non_empty: Vec<&Stmt> = block
        .stmts
        .iter()
        .filter(|s| !matches!(s, Stmt::Item(syn::Item::Verbatim(v)) if v.is_empty()))
        .collect();
    if non_empty.is_empty() {
        return None;
    }
    let span = block.span();
    let msg = "#[topo_pipeline] requires an empty body `{}` -- the IR \
               pass replaces the function body wholesale, so any \
               user-written statements are silently dropped at runtime";
    Some(quote_spanned! { span => compile_error!(#msg); })
}
