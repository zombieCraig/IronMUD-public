import axios, { AxiosError } from "axios";
export class IronMUDApiClient {
    client;
    constructor(baseUrl, apiKey) {
        this.client = axios.create({
            baseURL: baseUrl,
            headers: {
                Authorization: `Bearer ${apiKey}`,
                "Content-Type": "application/json",
            },
        });
    }
    async request(method, path, data) {
        try {
            const response = await this.client.request({
                method,
                url: path,
                data,
            });
            if (!response.data.success) {
                throw new Error(response.data.error?.message || "Unknown API error");
            }
            return response.data.data;
        }
        catch (error) {
            if (error instanceof AxiosError) {
                const apiError = error.response?.data?.error;
                throw new Error(apiError?.message || error.message || "API request failed");
            }
            throw error;
        }
    }
    async requestWithMeta(method, path, data) {
        try {
            const response = await this.client.request({
                method,
                url: path,
                data,
            });
            if (!response.data.success) {
                throw new Error(response.data.error?.message || "Unknown API error");
            }
            return {
                data: response.data.data,
                refreshed_instances: response.data.refreshed_instances,
            };
        }
        catch (error) {
            if (error instanceof AxiosError) {
                const apiError = error.response?.data?.error;
                throw new Error(apiError?.message || error.message || "API request failed");
            }
            throw error;
        }
    }
    async listRequest(path) {
        try {
            const response = await this.client.get(path);
            if (!response.data.success) {
                throw new Error("Failed to list entities");
            }
            return response.data.data;
        }
        catch (error) {
            if (error instanceof AxiosError) {
                const apiError = error.response?.data?.error;
                throw new Error(apiError?.message || error.message || "API request failed");
            }
            throw error;
        }
    }
    // Health check
    async health() {
        const response = await this.client.get("/health");
        return response.data;
    }
    // Areas
    async listAreas() {
        return this.listRequest("/areas");
    }
    async getArea(id) {
        return this.request("get", `/areas/${id}`);
    }
    async getAreaByPrefix(prefix) {
        return this.request("get", `/areas/by-prefix/${prefix}`);
    }
    async createArea(data) {
        return this.request("post", "/areas", data);
    }
    async updateArea(id, data) {
        return this.request("put", `/areas/${id}`, data);
    }
    async deleteArea(id) {
        await this.request("delete", `/areas/${id}`);
    }
    async resetArea(id) {
        return this.request("post", `/areas/${id}/reset`);
    }
    async listAreaRooms(areaId) {
        return this.listRequest(`/areas/${areaId}/rooms`);
    }
    async getAreaOverview(areaId) {
        return this.request("get", `/areas/${areaId}/overview`);
    }
    // Rooms
    async listRooms(limit, offset) {
        const params = new URLSearchParams();
        if (limit !== undefined)
            params.set("limit", limit.toString());
        if (offset !== undefined)
            params.set("offset", offset.toString());
        const query = params.toString();
        return this.listRequest(`/rooms${query ? `?${query}` : ""}`);
    }
    async listRoomsSummary(areaId, vnumPrefix) {
        const params = new URLSearchParams();
        if (areaId)
            params.set("area_id", areaId);
        if (vnumPrefix)
            params.set("vnum_prefix", vnumPrefix);
        const query = params.toString();
        return this.listRequest(`/rooms/summary${query ? `?${query}` : ""}`);
    }
    async getRoom(id) {
        return this.request("get", `/rooms/${id}`);
    }
    async getRoomByVnum(vnum) {
        return this.request("get", `/rooms/by-vnum/${encodeURIComponent(vnum)}`);
    }
    async createRoom(data) {
        return this.request("post", "/rooms", data);
    }
    async updateRoom(id, data) {
        return this.request("put", `/rooms/${id}`, data);
    }
    async deleteRoom(id) {
        await this.request("delete", `/rooms/${id}`);
    }
    async setRoomExit(roomId, direction, data) {
        return this.request("put", `/rooms/${roomId}/exits/${direction}`, data);
    }
    async removeRoomExit(roomId, direction) {
        return this.request("delete", `/rooms/${roomId}/exits/${direction}`);
    }
    async addRoomDoor(roomId, direction, data) {
        return this.request("put", `/rooms/${roomId}/doors/${direction}`, data);
    }
    async removeRoomDoor(roomId, direction) {
        return this.request("delete", `/rooms/${roomId}/doors/${direction}`);
    }
    async addRoomTrigger(roomId, data) {
        return this.request("post", `/rooms/${roomId}/triggers`, data);
    }
    async removeRoomTrigger(roomId, index) {
        return this.request("delete", `/rooms/${roomId}/triggers/${index}`);
    }
    async addRoomExtraDesc(roomId, data) {
        return this.request("post", `/rooms/${roomId}/extra`, data);
    }
    async removeRoomExtraDesc(roomId, keyword) {
        return this.request("delete", `/rooms/${roomId}/extra/${encodeURIComponent(keyword)}`);
    }
    // Items
    async listItems(limit, offset, itemType) {
        const params = new URLSearchParams();
        if (limit !== undefined)
            params.set("limit", limit.toString());
        if (offset !== undefined)
            params.set("offset", offset.toString());
        if (itemType)
            params.set("item_type", itemType);
        const query = params.toString();
        return this.listRequest(`/items${query ? `?${query}` : ""}`);
    }
    async listItemPrototypes() {
        return this.listRequest("/items/prototypes");
    }
    async listItemPrototypesSummary(vnumPrefix) {
        const params = new URLSearchParams();
        if (vnumPrefix)
            params.set("vnum_prefix", vnumPrefix);
        const query = params.toString();
        return this.listRequest(`/items/prototypes/summary${query ? `?${query}` : ""}`);
    }
    async getItem(id) {
        return this.request("get", `/items/${id}`);
    }
    async getItemByVnum(vnum) {
        return this.request("get", `/items/by-vnum/${encodeURIComponent(vnum)}`);
    }
    async createItem(data) {
        return this.request("post", "/items", data);
    }
    async updateItem(id, data) {
        return this.requestWithMeta("put", `/items/${id}`, data);
    }
    async deleteItem(id) {
        await this.request("delete", `/items/${id}`);
    }
    async spawnItem(vnum, data) {
        return this.request("post", `/items/${encodeURIComponent(vnum)}/spawn`, data);
    }
    async addItemTrigger(itemId, data) {
        return this.requestWithMeta("post", `/items/${itemId}/triggers`, data);
    }
    async removeItemTrigger(itemId, index) {
        return this.requestWithMeta("delete", `/items/${itemId}/triggers/${index}`);
    }
    // Mobiles
    async listMobiles(limit, offset) {
        const params = new URLSearchParams();
        if (limit !== undefined)
            params.set("limit", limit.toString());
        if (offset !== undefined)
            params.set("offset", offset.toString());
        const query = params.toString();
        return this.listRequest(`/mobiles${query ? `?${query}` : ""}`);
    }
    async listMobilePrototypes() {
        return this.listRequest("/mobiles/prototypes");
    }
    async listMobilePrototypesSummary(vnumPrefix) {
        const params = new URLSearchParams();
        if (vnumPrefix)
            params.set("vnum_prefix", vnumPrefix);
        const query = params.toString();
        return this.listRequest(`/mobiles/prototypes/summary${query ? `?${query}` : ""}`);
    }
    async getMobile(id) {
        return this.request("get", `/mobiles/${id}`);
    }
    async getMobileByVnum(vnum) {
        return this.request("get", `/mobiles/by-vnum/${encodeURIComponent(vnum)}`);
    }
    async createMobile(data) {
        return this.request("post", "/mobiles", data);
    }
    async updateMobile(id, data) {
        return this.requestWithMeta("put", `/mobiles/${id}`, data);
    }
    async deleteMobile(id) {
        await this.request("delete", `/mobiles/${id}`);
    }
    async addMobileDialogue(mobileId, data) {
        return this.requestWithMeta("post", `/mobiles/${mobileId}/dialogue`, data);
    }
    async removeMobileDialogue(mobileId, keyword) {
        return this.requestWithMeta("delete", `/mobiles/${mobileId}/dialogue/${encodeURIComponent(keyword)}`);
    }
    async addMobileRoutine(mobileId, data) {
        return this.requestWithMeta("post", `/mobiles/${mobileId}/routine`, data);
    }
    async removeMobileRoutine(mobileId, index) {
        return this.requestWithMeta("delete", `/mobiles/${mobileId}/routine/${index}`);
    }
    async addMobileTrigger(mobileId, data) {
        return this.requestWithMeta("post", `/mobiles/${mobileId}/triggers`, data);
    }
    async removeMobileTrigger(mobileId, index) {
        return this.requestWithMeta("delete", `/mobiles/${mobileId}/triggers/${index}`);
    }
    async spawnMobile(vnum, data) {
        return this.request("post", `/mobiles/${encodeURIComponent(vnum)}/spawn`, data);
    }
    // Spawn Points
    async listSpawnPoints(areaId) {
        const params = new URLSearchParams();
        if (areaId)
            params.set("area_id", areaId);
        const query = params.toString();
        return this.listRequest(`/spawn-points${query ? `?${query}` : ""}`);
    }
    async getSpawnPoint(id) {
        return this.request("get", `/spawn-points/${id}`);
    }
    async createSpawnPoint(data) {
        return this.request("post", "/spawn-points", data);
    }
    async updateSpawnPoint(id, data) {
        return this.request("put", `/spawn-points/${id}`, data);
    }
    async deleteSpawnPoint(id) {
        await this.request("delete", `/spawn-points/${id}`);
    }
    async addSpawnDependency(spawnPointId, data) {
        return this.request("post", `/spawn-points/${spawnPointId}/dependencies`, data);
    }
    async removeSpawnDependency(spawnPointId, index) {
        return this.request("delete", `/spawn-points/${spawnPointId}/dependencies/${index}`);
    }
    // Transports
    async listTransports() {
        return this.listRequest("/transports");
    }
    async getTransport(id) {
        return this.request("get", `/transports/${encodeURIComponent(id)}`);
    }
    async createTransport(data) {
        return this.request("post", "/transports", data);
    }
    async updateTransport(id, data) {
        return this.request("put", `/transports/${encodeURIComponent(id)}`, data);
    }
    async deleteTransport(id) {
        await this.request("delete", `/transports/${encodeURIComponent(id)}`);
    }
    async addTransportStop(transportId, data) {
        return this.request("post", `/transports/${encodeURIComponent(transportId)}/stops`, data);
    }
    async removeTransportStop(transportId, index) {
        return this.request("delete", `/transports/${encodeURIComponent(transportId)}/stops/${index}`);
    }
    async connectTransport(transportId, stopIndex) {
        return this.request("post", `/transports/${encodeURIComponent(transportId)}/connect`, { stop_index: stopIndex });
    }
    async startTransportTravel(transportId, destinationIndex) {
        return this.request("post", `/transports/${encodeURIComponent(transportId)}/travel`, { destination_index: destinationIndex });
    }
    // Plant Prototypes
    async listPlantPrototypes() {
        return this.listRequest("/plants");
    }
    async getPlantPrototype(id) {
        return this.request("get", `/plants/${id}`);
    }
    async getPlantPrototypeByVnum(vnum) {
        return this.request("get", `/plants/by-vnum/${encodeURIComponent(vnum)}`);
    }
    async createPlantPrototype(data) {
        return this.request("post", "/plants", data);
    }
    async updatePlantPrototype(id, data) {
        return this.request("put", `/plants/${id}`, data);
    }
    async deletePlantPrototype(id) {
        await this.request("delete", `/plants/${id}`);
    }
    // Bug Reports (approved only - see admin approval gate)
    async listBugReports(status) {
        const params = new URLSearchParams();
        if (status)
            params.set("status", status);
        const query = params.toString();
        return this.listRequest(`/bugs${query ? `?${query}` : ""}`);
    }
    async getBugReport(id) {
        return this.request("get", `/bugs/${id}`);
    }
    async getBugReportByTicket(num) {
        return this.request("get", `/bugs/by-ticket/${num}`);
    }
    async updateBugReport(id, data) {
        return this.request("put", `/bugs/${id}`, data);
    }
    async addBugNote(id, data) {
        return this.request("post", `/bugs/${id}/notes`, data);
    }
    async deleteBugReport(id) {
        await this.request("delete", `/bugs/${id}`);
    }
    // Helper methods for description context gathering
    /**
     * Get all rooms connected to a room via exits
     */
    async getConnectedRooms(roomId) {
        const room = await this.getRoom(roomId);
        const connected = [];
        const directions = ["north", "south", "east", "west", "up", "down"];
        for (const dir of directions) {
            const targetId = room.exits[dir];
            if (targetId) {
                try {
                    const targetRoom = await this.getRoom(targetId);
                    connected.push({ direction: dir, room: targetRoom });
                }
                catch {
                    // Target room may not exist, skip
                }
            }
        }
        return connected;
    }
    /**
     * Search for entities of a type with optional filters
     */
    async searchItems(filter) {
        let items = await this.listItems(filter?.limit || 50, 0, filter?.item_type);
        if (filter?.area_prefix) {
            items = items.filter((item) => item.vnum && item.vnum.startsWith(filter.area_prefix + ":"));
        }
        if (filter?.has_flag) {
            const flag = filter.has_flag;
            items = items.filter((item) => item.flags[flag]);
        }
        return items.slice(0, filter?.limit || 10);
    }
    async searchMobiles(filter) {
        let mobiles = await this.listMobiles(filter?.limit || 50);
        if (filter?.area_prefix) {
            mobiles = mobiles.filter((mobile) => mobile.vnum && mobile.vnum.startsWith(filter.area_prefix + ":"));
        }
        if (filter?.has_flag) {
            const flag = filter.has_flag;
            mobiles = mobiles.filter((mobile) => mobile.flags[flag]);
        }
        return mobiles.slice(0, filter?.limit || 10);
    }
    async searchRooms(filter) {
        let rooms = await this.listRooms(filter?.limit || 50);
        if (filter?.area_prefix) {
            rooms = rooms.filter((room) => room.vnum && room.vnum.startsWith(filter.area_prefix + ":"));
        }
        if (filter?.has_flag) {
            const flag = filter.has_flag;
            rooms = rooms.filter((room) => room.flags[flag]);
        }
        return rooms.slice(0, filter?.limit || 10);
    }
}
//# sourceMappingURL=api-client.js.map