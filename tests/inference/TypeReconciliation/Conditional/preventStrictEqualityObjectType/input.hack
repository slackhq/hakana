final class A {}
function foo(A $a, A $b) : bool {
    return $a === $b;
}
function foo2(A $a, A $b) : bool {
    return $a !== $b;
}
$a = foo(new A(), new A());
