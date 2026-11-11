// Technically the PublicResolver but it implements the Resolver spec
#[allow(non_snake_case)]
pub mod IResolver {
    use ethers::prelude::abigen;

    abigen!(Resolver, "./src/abi/resolver.json");
}
