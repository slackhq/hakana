<<__Sealed(B::class)>>
class A {
    public function __construct(private string $s) {}
}

final class B extends A {
    public function __construct(private string $s) {}
}

function foo(bool $b) : vec<A> {
    $classmap = $b ? dict[
        nameof A => 'bar',
        nameof B => 'baz'
    ] : dict[
        nameof A => 'foo',
    ];

    $v = vec[];
    foreach($classmap as $cls => $param) {
        $v[] = new $cls($param);
    }

    return $v;
}