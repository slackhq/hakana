class A {}
class B extends A {}

function foo(A $a): B {
    return $a;
}