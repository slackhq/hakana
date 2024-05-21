final class A {
    public readonly string $bar;

    public function __construct() {
        $this->bar = "hello";
    }
}

echo (new A())->bar;