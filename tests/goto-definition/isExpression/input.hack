interface MyInterface {
    public function getData(): string;
}

final class MyClass implements MyInterface {
    public function getData(): string {
        return "data";
    }
}

type MyAlias = MyClass;

function test_is_expression(mixed $value): void {
    if ($value is MyClass) {
        $value->getData();
    }
    if ($value is MyInterface) {
        $value->getData();
    }
    if ($value is MyAlias) {
        $value->getData();
    }
}
