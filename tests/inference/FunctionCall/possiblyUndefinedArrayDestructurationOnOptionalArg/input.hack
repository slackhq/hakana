final class A
{
}

function foo(A $a1, A $a2 = null): void
{
}

$arguments = vec[new A()];
if (mt_rand(1, 10) > 5) {
    // when this is done outside if - no errors
    $arguments[] = new A();
}

foo(...$arguments);
