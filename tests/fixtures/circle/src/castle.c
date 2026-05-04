/* Test fixture for the IronMUD CircleMUD castle.c importer.
 * castle_mob_spec() takes an OFFSET from the zone's bot vnum, derived
 * from the Z_KINGS_C define (offset = N*100). Stock 3.1 uses 150 → 15000;
 * we tag the fixture as zone 90 → 9000 so the binding resolves against
 * fixture mob #9002.
 */

#define Z_KINGS_C 90

void assign_kings_castle(void) {
  castle_mob_spec(2, king_welmar);
}
