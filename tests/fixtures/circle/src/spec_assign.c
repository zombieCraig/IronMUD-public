/* Test fixture for the IronMUD CircleMUD specproc importer.
 * Mirrors stock spec_assign.c shape: literal ASSIGNMOB/OBJ/ROOM lines,
 * comments, and the runtime ROOM_DEATH loop guarded by dts_are_dumps
 * (which the importer must NOT treat as a literal binding).
 */

void assign_mobiles(void) {
  ASSIGNMOB(9001, cityguard);     /* maps to MobileFlags.guard */
  ASSIGNMOB(9002, puff);           /* OnIdle @say_random with extracted quotes */
  ASSIGNMOB(9003, magic_user);     /* warn-only (no analog) */
  ASSIGNMOB(99999, cityguard);     /* orphan: vnum not in fixture */
  // ASSIGNMOB(8888, snake);       /* commented-out, must be ignored */
  /* ASSIGNMOB(7777, snake);       block-commented, must be ignored */
}

void assign_objects(void) {
  ASSIGNOBJ(9010, bank);           /* maps to OnUse @message */
}

void assign_rooms(void) {
  ASSIGNROOM(9001, dump);          /* maps to Periodic @room_message */

  /* The runtime DT-as-dump loop must NOT match the importer's pattern:
   * it iterates over flagged rooms at boot rather than naming a vnum.
   */
  if (dts_are_dumps)
    for (i = 0; i <= top_of_world; i++)
      if (ROOM_FLAGGED(i, ROOM_DEATH))
        world[i].func = dump;
}
