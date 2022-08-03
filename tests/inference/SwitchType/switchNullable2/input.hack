function foo(?string $s) : void {
    switch ($s) {
        case "hello":
            echo "cool";
        case "goodbye":
            echo "cooler";
            break;
        case "hello again":
            echo "cool";
            break;
    }
}