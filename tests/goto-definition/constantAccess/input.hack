class MyClass {
    const string MY_CONSTANT = "value";
}

function test(): void {
    echo MyClass::MY_CONSTANT; // Should go to definition on line 2
}