function breakUpPathIntoParts(): void {
    $b = false;

    while (rand(0, 1)) {
        if ($b) {
            if (rand(0, 1)) {
                $b = 0;
            }

            echo "hello";

            continue;
        }

        $b = true;
    }
}