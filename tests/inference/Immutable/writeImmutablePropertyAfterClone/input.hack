<<Hakana\Immutable>>
class A {
    public function __construct(public int $b) {}

    public function mutate(): A {
        $a = clone $this;
        $a->b = 5;
        return $a;
    }
}

<<__EntryPoint>>
function main() {
    $a = new A(3);
    $a->b = 6;
    bar($a);
}

function bar(A $a): A {
    $a = clone $a;
    $a->b = 7;
    return $a;
}