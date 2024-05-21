final class A<T as arraykey> {
    public function __construct(private T $t) {}

    public function foo() {
        if ($this->t is string && HH\Lib\Str\starts_with($this->t, "foo")) {
            echo "good";
        }
    }
}