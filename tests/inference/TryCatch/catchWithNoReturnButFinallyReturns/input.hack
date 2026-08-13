function foo() : bool {
    try {
        if (rand(0, 1) !== 0) throw new Exception("bad");
    } catch (Exception $e) {
        echo $e->getMessage();
        // do nothing here either
    } finally {
        return true;
    }
}