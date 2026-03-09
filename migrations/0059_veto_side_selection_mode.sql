-- Add side_selection_mode to veto_sessions
-- Controls how starting sides are determined for picked maps:
--   picker_choice: picker selects CT/T, decider maps skip
--   coin_flip: random side assignment after each pick
--   knife: no veto-level side selection, decided in-game

ALTER TABLE veto_sessions
  ADD COLUMN side_selection_mode VARCHAR(32) NOT NULL DEFAULT 'knife';
