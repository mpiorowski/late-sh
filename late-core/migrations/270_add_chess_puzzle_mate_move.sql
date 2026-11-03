-- A puzzle can end on a checkmate the pack does not list. The row keeps
-- that move (cozy-chess spelling), so a reconnect replays the mate the
-- player found and the share card counts only the moves they made. NULL on
-- every puzzle that followed the line.
ALTER TABLE chess_puzzle_games ADD COLUMN mate_move VARCHAR;
