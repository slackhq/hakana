$s = rand(0, 1) ? 0 : 1;

if ($s && rand(0, 1)) {
    if (rand(0, 1)) {
        $s = 2;
    }
}

if ($s == 2) {}