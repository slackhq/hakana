$context = 'a';
while ( true ) {
    if (rand(0, 1) !== 0) {
        if (rand(0, 1) !== 0) {
            exit();
        }

        $context = 'b';
    } else if (rand(0, 1) !== 0) {
        if ($context !== 'c' && $context !== 'b') {
            exit();
        }

        $context = 'c';
    }
}
