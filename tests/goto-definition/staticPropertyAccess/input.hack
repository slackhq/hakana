class MyClass {
    public static string $staticProperty = "value";
}

function test(): void {
    echo MyClass::$staticProperty; // Should go to definition on line 2
}