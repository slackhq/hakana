class MyClass {
    public static function staticMethod(): void {
        echo "Static method called";
    }
}

function test(): void {
    MyClass::staticMethod(); // Should go to definition on line 2
}