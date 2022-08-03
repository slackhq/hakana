function foo(?string $s, string $a, string $b) : void {
    if ($s == $a || $s == $b) {
        if ($s == $a) {
            echo "cool";
        }
        echo "cooler";
    }
}