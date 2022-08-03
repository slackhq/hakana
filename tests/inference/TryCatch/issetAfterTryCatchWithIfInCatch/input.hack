function test(): string {
    throw new Exception("bad");
}

function foo() : void {
    $a = null;

    $params = null;

    try {
        $a = test();

        $params = $a;
    } catch (\Exception $exception) {
        $params = "hello";
    }

    echo $params;
}