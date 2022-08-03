function foo(?string $s) : void {
    switch ($s) {
        case "hello":
        case "goodbye":
            echo "cool";
            break;
        case "hello again":
            echo "cool";
            break;
    }
}