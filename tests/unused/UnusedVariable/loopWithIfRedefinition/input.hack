$i = false;

foreach (vec[1, 2, 3] as $a) {
    if (rand(0, 1)) {
        $i = true;
    }

    echo $a;
}

if ($i) {}