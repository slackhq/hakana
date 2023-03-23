final class A<Tx as arraykey, Ty as arraykey> {}

function foo(): A<string, string> {
    return new A<string, string>();
}