final class A {
    public bool $b = false;
}

function foo(A $a, A $b): void {
    $c = !$a->b && !$b->b;
    echo $a->b ? 1 : 0;
}