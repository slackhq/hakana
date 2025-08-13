class MyClass {
    public function doSomething(): void {
        echo "Method called";
    }
}

function test(): void {
    $obj = new MyClass();
    $obj->doSomething(); // Should go to definition on line 2
}