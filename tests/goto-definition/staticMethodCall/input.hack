class MyClass {
    public static function staticMethod(): void {
        echo "Static method called";
    }

    public static function bar(): void {
        self::staticMethod();
    }
}

function test(): void {
    MyClass::staticMethod(); // Should go to definition on line 2
}