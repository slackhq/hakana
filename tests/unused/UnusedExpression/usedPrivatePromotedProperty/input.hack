final class A {
    private function __construct(private int $i) {}

    public static function gen(): A {
        return new A(5);
    }

    public function show(): void {
        echo $this->i;
    }
}

<<__EntryPoint>>
function main(): void {
    $a = A::gen();
    $a->show();
}
