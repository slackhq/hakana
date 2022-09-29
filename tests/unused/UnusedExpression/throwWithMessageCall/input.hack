function dangerous(): void {
    throw new \Exception("bad");
}

function callDangerous(): void {
    try {
        dangerous();
    } catch (Exception $e) {
        echo $e->getMessage();
    }
}