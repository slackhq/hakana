function foo(): void {
    $done = false;

    while (!$done) {
        try {
            $done = true;
        } catch (\Exception $e) {
        }
    }
}