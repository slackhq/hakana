$foo = null;
while (rand(0, 1)) {
    if (rand(0, 1)) {
        $foo = 1;
        continue;
    }

    $a = rand(0, 1);

    if ($a === $foo) {}
}