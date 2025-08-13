final class MyClass {
    public function getValue(): int {
        return 42;
    }
}

function test_function(MyClass $param): MyClass {
    return $param->getValue();
}