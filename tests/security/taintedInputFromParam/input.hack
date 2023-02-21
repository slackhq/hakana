class A {
    public function getUserId() : string {
        $user_id = (string) $_GET["user_id"];
        return $user_id;
    }

    public function getAppendedUserId() : string {
        $appended = "aaaa" . $this->getUserId();
        return $appended;
    }

    public function doDelete(AsyncMysqlConnection $conn) : void {
        $userId = $this->getAppendedUserId();
        $this->deleteUser($conn, $userId);
    }

    public function deleteUser(AsyncMysqlConnection $conn, string $userId) : void {
        $conn->query("delete from users where user_id = " . $userId);
    }
}