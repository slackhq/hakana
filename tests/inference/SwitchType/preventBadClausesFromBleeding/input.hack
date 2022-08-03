function foo (string $s) : void {
    if ($s === "a" && rand(0, 1)) {

    } else if ($s === "b" && rand(0, 1)) {

    } else {
        return;
    }

    switch ($s) {
        case "a":
            echo "hello";
            break;
        case "b":
            echo "goodbye";
            break;
    }
}