<<Hakana\Immutable>>
class A {
    public function __construct(public int $b) {}

    public function mutate() {
        $this->b = 5;
    }
}

<<__EntryPoint>>
function main() {
    $a = new A(3);
    $a->b = 6;
    bar($a);
}

function bar(A $a): void {
    $a->b = 7;
}