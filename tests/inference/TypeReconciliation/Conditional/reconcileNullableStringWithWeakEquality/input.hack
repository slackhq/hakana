function foo(?string $s) : void {
    if ($s == "hello" || $s == "goodbye") {
        if ($s == "hello") {
            echo "cool";
        }
        echo "cooler";
    }
}