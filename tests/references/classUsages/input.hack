class MyClass {
    public function doSomething(): void {}
}

function createInstance(): MyClass {
    return new MyClass();
}

function typeHint(MyClass $obj): void {
    $obj->doSomething();
}

function useAsReturn(): ?MyClass {
    return null;
}
