<<Hakana\Immutable>>
trait T {
	public int $b;
    public function __construct(int $b) {
        $this->b = $b; // ok
    }
}

final class A {
    use T;

    public function mutate() {
        $this->b = 5; // gets flagged
    }
}

<<__EntryPoint>>
function main() {
    $a = new A(3);
    $a->b = 6; // ok
    bar($a);
}

function bar(A $a): void {
    $a->b = 7; // gets flagged 
}