$a = false;

if (rand(0, 1) !== 0) {
    while (rand(0, 1)) {
        if (rand(0, 1) !== 0) {
            $a = true;
            break;
        }
    }
}

if ($a) {}
