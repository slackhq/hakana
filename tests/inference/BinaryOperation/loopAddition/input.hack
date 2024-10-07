function foo(): void {
    $p1 = 0;

    while (rand(0, 1)) {
        if (rand(0, 1)) {
            if ($p1 === 0) {
                bar($p1);
            } else {
                $p1 = $p1 - 1;
            }
        }

        if (rand(0, 1)) {
            $p1 = $p1 - 1;
        }
    }

    bar($p1);
}

function bar(int $i): void {}