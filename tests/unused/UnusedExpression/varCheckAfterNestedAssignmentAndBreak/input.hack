$a = false;

if (rand(0, 1) !== 0) {
    while (rand(0, 1)) {
        $a = true;
        break;
    }
}

if ($a) {}
