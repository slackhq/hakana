class A {
    public function getUserId() : string {
        return (string) $_GET["user_id"];
    }

    public function getAppendedUserId() : string {
        return "aaaa" . $this->getUserId();
    }

    public function deleteUser(AsyncMysqlConnection $conn) : void {
        $userId = $this->getAppendedUserId();
        $conn->query("delete from users where user_id = " . $userId);
    }
}