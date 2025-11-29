final class MyClass {
    public function getValue(): int {
        return 42;
    }
}

final class Container {
    public MyClass $property;
    public static MyClass $staticProperty;

    public function __construct() {
        $this->property = new MyClass();
        self::$staticProperty = new MyClass();
    }
}
