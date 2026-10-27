function breakUpPathIntoParts(): void {
    $b = false;

    while (rand(0, 1) !== 0) {
        if ($b) {
            if (rand(0, 1) !== 0) {
                $b = false;
            }

            echo "hello";

            continue;
        }

        $b = true;
    }
}
