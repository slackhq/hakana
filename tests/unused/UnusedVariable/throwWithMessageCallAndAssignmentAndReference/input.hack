function dangerous(): string {
    if (rand(0, 1)) {
        throw new \Exception("bad");
    }

    return "hello";
}

function callDangerous(): void {
    $s = null;

    try {
        $s = dangerous();
    } catch (Exception $e) {
        echo $e->getMessage();
    }

    if ($s) {}
}