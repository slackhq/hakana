final class MyClass {
    public function getData(): string {
        return "data";
    }
}

type MyAlias = MyClass;

function test_as_expression(mixed $value): void {
    $result1 = $value as MyClass;
    $result1->getData();

    $result2 = $value as MyAlias;
    $result2->getData();

    $result3 = $value as ?MyClass;
    $result3?->getData();
}
