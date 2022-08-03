class C {
    public string $a = "";
    public string $b = "";
}

function testElse(C $obj) : void {
    if ($obj->a === "foo") {
    } else if ($obj->b === "bar") {
    } else if ($obj->b === "baz") {}

    if ($obj->b === "baz") {}
}