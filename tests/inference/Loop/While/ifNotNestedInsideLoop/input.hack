function analyse(): int {
    $state = 1;

    while (rand(0, 1)) {
        if ($state === 3) {
            echo "here";
        } else if ($state === 2) {
            $state = 3;
        } else {
            $state = 2;
        }
    }

    return $state;
}