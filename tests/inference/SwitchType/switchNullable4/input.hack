function foo(?string $s, string $a, string $b) : void {
    switch ($s) {
        case $a:
        case $b:
            break;
    }
}