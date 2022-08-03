class A {
    public function getUserId() : string {
        return (string) $_GET["user_id"];
    }

    public function getAppendedUserId() : string {
        return "aaaa" . $this->getUserId();
    }

    public function deleteUser(MySqlHandler $conn) : void {
        $userId = $this->getAppendedUserId();
        $conn->exec("delete from users where user_id = " . $userId);
    }
}

class MySqlHandler {
    public function exec(
        <<\Hakana\SecurityAnalysis\Sink("sql")>> string $sql
    ) : void {}
}