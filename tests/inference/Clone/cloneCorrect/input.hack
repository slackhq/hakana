class A {}
function foo(A $a) : A {
    return clone $a;
}
$a = foo(new A());