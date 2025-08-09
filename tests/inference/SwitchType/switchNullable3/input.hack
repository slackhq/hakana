function foo(?string $s) : void {
    switch ($s) {
        case "hello":
            echo "cool";
            break;
        case "goodbye":
            echo "cool";
            break;
        case "hello again":
            echo "cool";
            break;
        case null:
            break;
    }
}