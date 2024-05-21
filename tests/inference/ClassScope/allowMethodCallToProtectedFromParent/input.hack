abstract class A {
    public function __construct() {
        B::foo();
    }
}

final class B extends A {
    protected static function foo(): void {
        echo "here";
    }
}