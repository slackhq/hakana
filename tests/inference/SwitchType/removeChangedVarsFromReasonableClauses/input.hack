function r() : bool {
    return (bool)rand(0, 1);
}

function foo(string $s) : void {
    if (($s === "a" || $s === "b")
        && ($s === "a" || r())
        && ($s === "b" || r())
        && (r() || r())
    ) {
        // do something
    } else {
        return;
    }

    switch ($s) {
        case "a":
            break;
        case "b":
            break;
    }
}