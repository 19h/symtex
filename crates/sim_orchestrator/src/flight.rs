// symtex/crates/sim_orchestrator/src/flight.rs
use crate::{metrics::Metrics, state::CanonicalState};
use arrow::record_batch::RecordBatch;
use arrow_array::{ArrayRef, LargeBinaryArray};
use arrow_flight::{
    flight_service_server::{FlightService, FlightServiceServer},
    utils::batches_to_flight_data,
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo,
    HandshakeRequest, HandshakeResponse, PollInfo, PutResult, SchemaResult, Ticket,
};
use arrow_schema::{DataType, Field, Schema};
use futures::Stream;
use std::{pin::Pin, sync::Arc};
use tonic::{Request, Response, Status};

/// Implements the Apache Arrow Flight service for serving reveal mask data.
pub struct FlightSvc {
    state: Arc<CanonicalState>,
    metrics: Arc<Metrics>,
}

#[tonic::async_trait]
impl FlightService for FlightSvc {
    type DoGetStream =
        Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send + 'static>>;

    /// Handles a client request to retrieve a data stream. In this service, it's used
    /// exclusively to fetch the reveal mask bitmap associated with a given ticket.
    async fn do_get(
        &self,
        req: Request<Ticket>,
    ) -> Result<Response<Self::DoGetStream>, Status> {
        self.metrics.flight_requests_total.inc();

        let ticket_bytes = req.into_inner().ticket;

        // 1. Validate the ticket and retrieve the corresponding data snapshot.
        let reveal_mask_snapshot = {
            let tickets = self.state.valid_flight_tickets.read();
            tickets
                .get(ticket_bytes.as_ref())
                .cloned()
                .ok_or_else(|| Status::not_found("Invalid or expired ticket"))?
        };

        // 2. Serialize the RoaringBitmap into its portable byte format.
        let mut buffer = Vec::new();
        reveal_mask_snapshot
            .serialize_into(&mut buffer)
            .map_err(|e| Status::internal(format!("Failed to serialize bitmap: {}", e)))?;

        // 3. Define the Arrow Schema for the data.
        let schema = Arc::new(Schema::new(vec![Field::new(
            "roaring_portable",
            DataType::LargeBinary,
            false,
        )
        .with_metadata(
            [
                ("content_type".to_string(), "application/x-roaring".to_string()),
                ("version".to_string(), "1".to_string()),
            ]
            .into(),
        )]));

        // 4. Create an Arrow RecordBatch containing the serialized data.
        let array: ArrayRef = Arc::new(LargeBinaryArray::from_iter_values([buffer]));
        let batch = RecordBatch::try_new(schema.clone(), vec![array])
            .map_err(|e| Status::internal(format!("Failed to create RecordBatch: {}", e)))?;

        // 5. Convert the RecordBatch into a sequence of FlightData messages.
        //    Output ordering: [Schema, (0..K dictionary messages), Batch]
        let flight_chunks: Vec<FlightData> = batches_to_flight_data(
            batch.schema().as_ref(),
            vec![batch],
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        let stream = futures::stream::iter(flight_chunks.into_iter().map(Ok));

        tracing::debug!(
            ticket_len = ticket_bytes.len(),
            points = reveal_mask_snapshot.len(),
            "Served Flight ticket"
        );

        Ok(Response::new(Box::pin(stream) as Self::DoGetStream))
    }

    // --- Unimplemented Service Methods ---

    async fn get_flight_info(
        &self,
        _req: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        Err(Status::unimplemented("GetFlightInfo not implemented"))
    }

    async fn poll_flight_info(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<PollInfo>, Status> {
        Err(Status::unimplemented("PollFlightInfo not implemented"))
    }

    type HandshakeStream =
        Pin<Box<dyn Stream<Item = Result<HandshakeResponse, Status>> + Send + 'static>>;
    async fn handshake(
        &self,
        _req: Request<tonic::Streaming<HandshakeRequest>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        Err(Status::unimplemented("Handshake not implemented"))
    }

    type ListFlightsStream =
        Pin<Box<dyn Stream<Item = Result<FlightInfo, Status>> + Send + 'static>>;
    async fn list_flights(
        &self,
        _req: Request<Criteria>,
    ) -> Result<Response<Self::ListFlightsStream>, Status> {
        Err(Status::unimplemented("ListFlights not implemented"))
    }

    async fn get_schema(
        &self,
        _req: Request<FlightDescriptor>,
    ) -> Result<Response<SchemaResult>, Status> {
        Err(Status::unimplemented("GetSchema not implemented"))
    }

    type DoPutStream = Pin<Box<dyn Stream<Item = Result<PutResult, Status>> + Send + 'static>>;
    async fn do_put(
        &self,
        _req: Request<tonic::Streaming<FlightData>>,
    ) -> Result<Response<Self::DoPutStream>, Status> {
        Err(Status::unimplemented("DoPut not implemented"))
    }

    type DoExchangeStream =
        Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send + 'static>>;
    async fn do_exchange(
        &self,
        _req: Request<tonic::Streaming<FlightData>>,
    ) -> Result<Response<Self::DoExchangeStream>, Status> {
        Err(Status::unimplemented("DoExchange not implemented"))
    }

    type DoActionStream =
        Pin<Box<dyn Stream<Item = Result<arrow_flight::Result, Status>> + Send + 'static>>;
    async fn do_action(
        &self,
        _req: Request<Action>,
    ) -> Result<Response<Self::DoActionStream>, Status> {
        Err(Status::unimplemented("DoAction not implemented"))
    }

    type ListActionsStream =
        Pin<Box<dyn Stream<Item = Result<ActionType, Status>> + Send + 'static>>;
    async fn list_actions(
        &self,
        _req: Request<Empty>,
    ) -> Result<Response<Self::ListActionsStream>, Status> {
        Err(Status::unimplemented("ListActions not implemented"))
    }
}

/// Factory function to create a new `FlightServiceServer`.
pub fn make_server(
    state: Arc<CanonicalState>,
    metrics: Arc<Metrics>,
) -> FlightServiceServer<FlightSvc> {
    FlightServiceServer::new(FlightSvc { state, metrics })
}
