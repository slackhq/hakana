function dangerous(): string {
    if (rand(0, 1) !== 0) {
        throw new \Exception("bad");
    }

    return "hello";
}

function callDangerous(): void {
    $s = null;

    try {
        dangerous();
    } catch (Exception $e) {
        echo $e->getMessage();
        $s = "hello";
    }

    if ($s) {}
}
