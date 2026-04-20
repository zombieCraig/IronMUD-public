# Transport Editor (tedit)

The transport system allows builders to create elevators, buses, trains, and other vehicles that move players between locations.

## Core Concepts

### Transport Types
- `elevator` - Vertical transport between floors (typically on-demand)
- `bus` - City transport with multiple stops
- `train` - Long-distance transport
- `ferry` - Water-based transport
- `cable_car` - Mountain/scenic transport
- `shuttle` - Simple back-and-forth transport

### Components
1. **Interior Room** - Where passengers wait during travel
2. **Stops** - Locations the transport visits, each with:
   - A room to connect to
   - A display name (e.g., "Lobby", "Floor 2")
   - An exit direction where the transport appears

### Schedule Modes
- **On-Demand** - Responds to button presses (elevators)
- **Game-Time** - Runs on a fixed schedule based on in-game hours

## Commands

### Creating and Managing Transports
```
tedit create <vnum>               - Create new transport
tedit <vnum>                      - Show transport properties
tedit <vnum> name <text>          - Set display name
tedit <vnum> type <type>          - Set transport type
tedit <vnum> interior <room_vnum> - Set interior room
tedit <vnum> traveltime <secs>    - Set travel time between stops
tedit <vnum> delete               - Delete transport
```

### Schedule Configuration
```
tedit <vnum> schedule ondemand    - On-demand mode (elevator style)
tedit <vnum> schedule gametime <freq> <start> <end> <dwell>
```

Game-time parameters:
- `freq` - Frequency in game hours (e.g., 1 = every hour)
- `start` - Start hour (0-23)
- `end` - End hour (0-23)
- `dwell` - Seconds to wait at each stop

### Stop Management
```
tedit <vnum> stop add <room_vnum> <name> <exit_dir>
tedit <vnum> stop remove <index>
tedit <vnum> stop list
```

### Connection Control
```
tedit <vnum> connect              - Connect to current stop
tedit <vnum> disconnect           - Disconnect from current stop
```

## Examples

### Creating an Elevator

```
# 1. Create the interior room (the elevator car)
redit create hotel:elevator_car
redit title Inside the Elevator
redit desc A small elevator car with polished brass walls...
redit flags indoors

# 2. Create lobby and floor rooms with appropriate exits
# (the elevator will appear from these exit directions)

# 3. Create the transport
tedit create hotel:elevator
tedit hotel:elevator name "Hotel Elevator"
tedit hotel:elevator type elevator
tedit hotel:elevator interior hotel:elevator_car
tedit hotel:elevator traveltime 3
tedit hotel:elevator schedule ondemand

# 4. Add stops
tedit hotel:elevator stop add hotel:lobby "Lobby" east
tedit hotel:elevator stop add hotel:floor2 "Floor 2" east
tedit hotel:elevator stop add hotel:floor3 "Floor 3" east

# 5. Connect the elevator to its first stop
tedit hotel:elevator connect
```

### Creating a City Bus

```
# 1. Create bus interior
redit create city:bus_interior
redit title On the Bus
redit desc Worn plastic seats line both sides of the bus...

# 2. Create the bus transport
tedit create city:bus_route1
tedit city:bus_route1 name "Route 1 Bus"
tedit city:bus_route1 type bus
tedit city:bus_route1 interior city:bus_interior
tedit city:bus_route1 traveltime 30

# 3. Set game-time schedule (every 2 hours, 6am-10pm, 60 sec dwell)
tedit city:bus_route1 schedule gametime 2 6 22 60

# 4. Add stops
tedit city:bus_route1 stop add city:downtown "Downtown Station" north
tedit city:bus_route1 stop add city:market "Market Square" east
tedit city:bus_route1 stop add city:harbor "Harbor Terminal" south

# 5. Connect
tedit city:bus_route1 connect
```

## Best Practices

1. **Interior Room Design**
   - Set `indoors` flag
   - Add atmospheric description
   - Consider adding a "schedule" extra description

2. **Stop Placement**
   - Choose exit directions that make sense (elevator = east/west, bus = any)
   - Ensure the connecting room exists before adding stop

3. **Travel Time**
   - Elevators: 2-5 seconds
   - Buses: 20-60 seconds between stops
   - Trains: 60-180 seconds

4. **On-Demand vs Scheduled**
   - Use on-demand for elevators and private shuttles
   - Use game-time for public transport with realistic schedules

## Troubleshooting

### "Transport doesn't appear"
- Verify the transport is connected (`tedit <vnum>` shows state)
- Check that stops are configured
- Ensure interior room exists

### "Can't enter transport"
- Verify you're in a room with a configured stop
- Check the exit direction matches the stop configuration
- Transport must be "stopped" at that location

### "Transport not moving"
- For on-demand: players must use the call button
- For game-time: check the schedule hours and frequency
