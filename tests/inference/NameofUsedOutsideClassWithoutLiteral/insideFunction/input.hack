final class C {
    public static function foo(): void {
        echo nameof C;
        echo nameof self;
        echo nameof static;
    }

    public function bar(): void {
        echo nameof C;
        echo nameof self;
        echo nameof static;
    }
}

function only_literal_classname_allowed_here(): void {
    echo nameof C;
    echo nameof self;
    echo nameof static;
    echo nameof parent;
}