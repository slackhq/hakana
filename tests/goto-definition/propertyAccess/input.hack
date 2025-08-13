class MyClass {
    public string $myProperty = "test";
}

function test(): void {
    $obj = new MyClass();
    echo $obj->myProperty; // Should go to definition on line 2
}