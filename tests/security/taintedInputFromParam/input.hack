class A {
    public function getUserId() : string {
        return (string) $_GET["user_id"];
    }

    public function getAppendedUserId() : string {
        return "aaaa" . $this->getUserId();
    }

    public function doDelete(AsyncMysqlConnection $conn) : void {
        $userId = $this->getAppendedUserId();
        $this->deleteUser($conn, $userId);
    }

    public function deleteUser(AsyncMysqlConnection $conn, string $userId) : void {
        $conn->query("delete from users where user_id = " . $userId);
    }
}