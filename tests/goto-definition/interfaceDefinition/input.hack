interface IMyInterface {
    public function getData(): string;
}

class Implementation implements IMyInterface {
    public function getData(): string {
        return "data";
    }
}

function test_interface(): void {
    $impl = new Implementation();
    if ($impl is IMyInterface) {
        $impl->getData();
    }
}