function foo(): void {
    $a = rand(0, 1) !== 0 ? "a" : "b";

    switch ($a) {
        case "a":
            $foo = "hello";
            break;

        case "b":
            $foo = "goodbye";
            break;
    }

    echo $foo;
}
