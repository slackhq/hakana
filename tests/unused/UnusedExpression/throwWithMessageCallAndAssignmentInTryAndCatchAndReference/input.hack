function dangerous(): string {
    if (rand(0, 1)) {
        throw new \Exception("bad");
    }

    return "hello";
}

function callDangerous(): void {
    try {
        $s = dangerous();
    } catch (Exception $e) {
        echo $e->getMessage();
        $s = "hello";
    }

    if ($s) {}
}