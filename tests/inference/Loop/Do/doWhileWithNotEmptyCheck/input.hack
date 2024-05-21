final class A {
    public ?A $a;

    public function __construct() {
        $this->a = rand(0, 1) ? new A() : null;
    }
}

function takesA(A $a): void {}

$a = new A();
do {
    takesA($a);
    $a = $a->a;
} while ($a);

if ($a is null) {}