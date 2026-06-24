function foo(): int {
    switch (true) {
        default:
            while (rand(0, 1)) {
                if (rand(0, 1) !== 0) {
                    continue;
                }
                return 1;
            }
            return 2;
    }
}
