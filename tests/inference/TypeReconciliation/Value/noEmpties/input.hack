$context = 'a';
while ( true ) {
    if (rand(0, 1)) {
        if (rand(0, 1)) {
            exit();
        }

        $context = 'b';
    } else if (rand(0, 1)) {
        if ($context !== 'c' && $context !== 'b') {
            exit();
        }

        $context = 'c';
    }
}