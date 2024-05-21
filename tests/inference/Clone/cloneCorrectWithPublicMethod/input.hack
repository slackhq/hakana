final class A {
    public function __clone() {}
}
function foo(A $a) : A {
    return clone $a;
}
foo(new A());