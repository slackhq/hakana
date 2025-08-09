function foo(?string $s) : void {
    switch ($s) {
        case "hello":
        case "goodbye":
        case null:
            echo "cool";
            break;
        case "hello again":
            echo "cool";
            break;
    }
}