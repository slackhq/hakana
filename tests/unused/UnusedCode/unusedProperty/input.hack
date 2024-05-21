final class A {
    private string $a = 'c';
    public int $b = 1;
    public function __construct(
        private string $c,
        public int $d = 2,
    ) {}
}

final class B {
    private string $a = 'c';
    public int $b = 1;
    public function __construct(
        private string $c,
        public int $d = 2,
    ) {}

    public function usePrivate(): void {
        echo $this->a;
        echo $this->c;
    }
}

<<__EntryPoint>>
function foo() {
    new A('c');
    $b = new B('c');
    $b->usePrivate();
    echo $b->b;
    echo $b->d;
}

