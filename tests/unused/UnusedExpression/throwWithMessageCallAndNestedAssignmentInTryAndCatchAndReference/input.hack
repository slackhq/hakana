function dangerous(): string {
    if (rand(0, 1) !== 0) {
        throw new \Exception("bad");
    }

    return "hello";
}

function callDangerous(): void {
    $s = null;

    if (rand(0, 1) !== 0) {
        $s = "hello";
    } else {
        try {
            $t = dangerous();
        } catch (Exception $e) {
            echo $e->getMessage();
            $t = "hello";
        }

        if ($t) {
            $s = $t;
        }
    }

    if ($s) {}
}
