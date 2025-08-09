function foo(?string $s) : void {
    switch ($s) {
        case "hello":
            $a = 1;
            break;
        case "bello":
            $a = 2;
            break;
        case "goodbye":
            throw new Exception('bad');
            break;
        case null:
            $a = 3;
            break;
    }
    echo $a;
}