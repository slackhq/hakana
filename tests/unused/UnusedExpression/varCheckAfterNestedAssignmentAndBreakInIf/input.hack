$a = false;

if (rand(0, 1)) {
    while (rand(0, 1)) {
        if (rand(0, 1)) {
            $a = true;
            break;
        }
    }
}

if ($a) {}